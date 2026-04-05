use {
    maud::{Markup, html},
    serde::{Deserialize, Serialize},
    std::collections::{BTreeMap, BTreeSet},
};

const REQUIRED_COLUMNS: [&str; 4] = ["id", "quantity", "head", "body"];

#[derive(Debug, Clone, Serialize)]
pub struct KanbanRenderedSync {
    pub html: KanbanRenderedHtml,
    pub summary: KanbanRenderedSummary,
    pub view: KanbanRenderedViewMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct KanbanRenderedHtml {
    pub header_meta: String,
    pub toolbar_state: String,
    pub columns: String,
    pub empty_or_error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KanbanRenderedSummary {
    pub row_count: usize,
    pub active_worklog_count: i64,
    pub filtered: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct KanbanRenderedViewMeta {
    pub view_id: u32,
    pub name: String,
    pub query: String,
    pub columns: Vec<String>,
}

#[derive(Debug)]
pub enum KanbanRenderError {
    InvalidPayload(String),
    ShapeMismatch {
        expected_columns: Vec<&'static str>,
        received_columns: Vec<String>,
    },
}

impl std::fmt::Display for KanbanRenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPayload(message) => f.write_str(message),
            Self::ShapeMismatch {
                expected_columns,
                received_columns,
            } => write!(
                f,
                "Snapshot mismatch. Expected columns {:?}, received {:?}.",
                expected_columns, received_columns
            ),
        }
    }
}

impl std::error::Error for KanbanRenderError {}

#[derive(Debug, Deserialize)]
struct ViewSnapshotPayload {
    view_id: u32,
    name: String,
    query: String,
    columns: Vec<String>,
    rows: Vec<BTreeMap<String, String>>,
}

#[derive(Debug, Clone)]
struct KanbanRow {
    id: i64,
    quantity: i64,
    head: String,
    body: String,
    task_type: Option<String>,
    primary_category: Option<String>,
    categories: Vec<String>,
    assignee_names: Vec<String>,
    parent_id: Option<i64>,
    parent_head: Option<String>,
    comments_count: Option<i64>,
    active_worklog_count: Option<i64>,
}

#[derive(Clone, Copy)]
struct Lane {
    key: &'static str,
    label: &'static str,
}

const LANES: [Lane; 5] = [
    Lane {
        key: "backlog",
        label: "Backlog",
    },
    Lane {
        key: "next",
        label: "Next",
    },
    Lane {
        key: "wip",
        label: "WIP",
    },
    Lane {
        key: "review",
        label: "Review",
    },
    Lane {
        key: "done",
        label: "Done",
    },
];

pub fn render_sync_payload(
    payload: &str,
    show_parent_context: bool,
) -> Result<KanbanRenderedSync, KanbanRenderError> {
    let snapshot = serde_json::from_str::<ViewSnapshotPayload>(payload)
        .map_err(|error| KanbanRenderError::InvalidPayload(error.to_string()))?;
    if !REQUIRED_COLUMNS
        .iter()
        .all(|column| snapshot.columns.iter().any(|value| value == column))
    {
        return Err(KanbanRenderError::ShapeMismatch {
            expected_columns: REQUIRED_COLUMNS.into(),
            received_columns: snapshot.columns.clone(),
        });
    }

    let rows = snapshot
        .rows
        .iter()
        .filter_map(parse_row)
        .map(|mut row| {
            if !show_parent_context {
                row.parent_id = None;
                row.parent_head = None;
            }
            row
        })
        .collect::<Vec<_>>();
    let active_worklog_count = rows
        .iter()
        .map(|row| row.active_worklog_count.unwrap_or(0))
        .sum::<i64>();
    let filtered = query_looks_filtered(&snapshot.query);

    Ok(KanbanRenderedSync {
        html: KanbanRenderedHtml {
            header_meta: render_header_meta(&snapshot).into_string(),
            toolbar_state: render_toolbar_state(rows.len(), active_worklog_count).into_string(),
            columns: render_columns(&rows).into_string(),
            empty_or_error: render_empty_state(rows.is_empty()).into_string(),
        },
        summary: KanbanRenderedSummary {
            row_count: rows.len(),
            active_worklog_count,
            filtered,
        },
        view: KanbanRenderedViewMeta {
            view_id: snapshot.view_id,
            name: snapshot.name,
            query: snapshot.query,
            columns: snapshot.columns,
        },
    })
}

pub fn render_error_payload(message: &str) -> KanbanRenderedHtml {
    KanbanRenderedHtml {
        header_meta: String::new(),
        toolbar_state: String::new(),
        columns: String::new(),
        empty_or_error: render_error_state(message).into_string(),
    }
}

pub fn render_mismatch_payload(
    expected_columns: &[&'static str],
    received_columns: &[String],
) -> KanbanRenderedHtml {
    KanbanRenderedHtml {
        header_meta: String::new(),
        toolbar_state: String::new(),
        columns: String::new(),
        empty_or_error: render_mismatch_state(expected_columns, received_columns).into_string(),
    }
}

fn parse_row(raw: &BTreeMap<String, String>) -> Option<KanbanRow> {
    Some(KanbanRow {
        id: parse_i64(raw.get("id")?)?,
        quantity: parse_i64(raw.get("quantity")?)?,
        head: normalize_text(raw.get("head")),
        body: normalize_text(raw.get("body")),
        task_type: normalize_optional(raw.get("task_type")),
        primary_category: normalize_optional(raw.get("primary_category")),
        categories: parse_json_strings(raw.get("categories_json")),
        assignee_names: parse_json_strings(raw.get("assignee_names_json")),
        parent_id: raw.get("parent_id").and_then(|value| parse_i64(value)),
        parent_head: normalize_optional(raw.get("parent_head")),
        comments_count: raw.get("comments_count").and_then(|value| parse_i64(value)),
        active_worklog_count: raw
            .get("active_worklog_count")
            .and_then(|value| parse_i64(value)),
    })
}

fn render_header_meta(snapshot: &ViewSnapshotPayload) -> Markup {
    html! {
        .headerTitle { (&snapshot.name) }
        .headerSub {
            "view "
            (snapshot.view_id)
            " · "
            (snapshot.query.as_str())
        }
    }
}

fn render_toolbar_state(row_count: usize, active_worklog_count: i64) -> Markup {
    html! {
        span.pill { "Rows " (row_count) }
        span.pill { "Active timers " (active_worklog_count) }
    }
}

fn render_columns(rows: &[KanbanRow]) -> Markup {
    html! {
        @for lane in LANES {
            (render_lane(lane, rows_for_lane(lane, rows)))
        }
    }
}

fn render_lane(lane: Lane, rows: Vec<&KanbanRow>) -> Markup {
    let collapsed_expr = format!("$ui.lanes.{}.collapsed", lane.key);
    let width_expr = format!(
        "{} ? '{}px' : ($ui.lanes.{}.width + 'px')",
        collapsed_expr.as_str(),
        64,
        lane.key
    );
    let toggle_expr = format!(
        "$ui.lanes.{}.collapsed = !$ui.lanes.{}.collapsed",
        lane.key, lane.key
    );
    let toggle_label_expr = format!("{} ? '+' : '-'", collapsed_expr.as_str());
    html! {
        section.col
            data-col=(lane.key)
            data-class:is-collapsed=(collapsed_expr.as_str())
            data-style:width=(width_expr.as_str())
            data-style:min-width=(width_expr.as_str())
            data-style:flex-basis=(width_expr.as_str())
        {
            button.colResizeEdge.colResizeEdge--left type="button" title="Resize column" data-resize-handle=(lane.key) data-resize-side="left" {}
            header.colHead {
                .colHeadMain {
                    button.laneToggle
                        type="button"
                        title={ "Toggle " (lane.label) }
                        data-on:click=(toggle_expr)
                        data-text=(toggle_label_expr)
                    { "-" }
                    .colName { (lane.label) }
                    .count { (rows.len()) }
                }
            }
            .list data-dropzone=(lane.key) data-show=(format!("!({})", collapsed_expr.as_str())) style="display: none" {
                @if rows.is_empty() {
                    .empty { "Drop records here" }
                } @else {
                    @for row in rows {
                        (render_card(row))
                    }
                }
            }
            button.colResizeEdge.colResizeEdge--right type="button" title="Resize column" data-resize-handle=(lane.key) data-resize-side="right" {}
        }
    }
}

fn render_card(row: &KanbanRow) -> Markup {
    let compact_body = compact_body(&row.body);
    let full_body = row.body.as_str();
    let categories = categories_for_row(row);
    let assignees = row.assignee_names.join(", ");
    let parent_label = row
        .parent_head
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| row.parent_id.map(|value| format!("Parent #{value}")));
    let card_class = format!("card {}", quantity_to_lane_key(row.quantity));
    let body_mode_expr = format!("($ui.cardModes['{}'] || $ui.defaultBodyMode)", row.id);
    let head_mode_expr = format!("{body_mode_expr} === 'head'");
    let compact_mode_expr = format!("{body_mode_expr} === 'compact'");
    let full_mode_expr = format!("{body_mode_expr} === 'full'");
    let set_head_expr = format!("$ui.cardModes['{}'] = 'head'", row.id);
    let set_compact_expr = format!("$ui.cardModes['{}'] = 'compact'", row.id);
    let set_full_expr = format!("$ui.cardModes['{}'] = 'full'", row.id);
    html! {
        article class=(card_class)
            draggable="true"
            data-record-id=(row.id)
            data-quantity=(row.quantity)
            data-head=(row.head.as_str())
            data-body=(full_body)
        {
            .cardActions {
                button.cardAction
                    type="button"
                    title="Show only the head"
                    data-on:click=(set_head_expr)
                    data-class:is-active=(head_mode_expr.as_str())
                { "_" }
                button.cardAction
                    type="button"
                    title="Show compact body"
                    data-on:click=(set_compact_expr)
                    data-class:is-active=(compact_mode_expr.as_str())
                { "=" }
                button.cardAction
                    type="button"
                    title="Show full body"
                    data-on:click=(set_full_expr)
                    data-class:is-active=(full_mode_expr.as_str())
                { "+" }
                button.cardAction type="button" data-open-focus=(row.id) data-on:click="window.KanbanWidget?.loadRecordDetail(Number(evt.currentTarget.dataset.openFocus || 0))" title="Focus card" { "[]" }
            }
            @if let Some(parent_label) = parent_label.as_deref() {
                @if let Some(parent_id) = row.parent_id {
                    a.cardParentLink
                        href="#"
                        data-record-link=(parent_id)
                        data-on:click__prevent="window.KanbanWidget?.loadRecordDetail(Number(evt.currentTarget.dataset.recordLink || 0))"
                    {
                        (parent_label)
                    }
                } @else {
                    span.cardParentLink {
                        (parent_label)
                    }
                }
            }
            button.head.headButton type="button" data-open-focus=(row.id) data-on:click="window.KanbanWidget?.loadRecordDetail(Number(evt.currentTarget.dataset.openFocus || 0))" {
                @if row.head.trim().is_empty() {
                    "(no head)"
                } @else {
                    (&row.head)
                }
            }
            .meta {
                span { "#" (row.id) }
                span { "qty " (row.quantity) }
                @if let Some(task_type) = row.task_type.as_deref() {
                    span { (task_type) }
                }
            }
            @if !categories.is_empty() {
                .tagRow {
                    @for category in &categories {
                        span.pill { (category) }
                    }
                }
            }
            @if !assignees.is_empty() {
                .small { "Assignees: " (assignees) }
            }
            @if !compact_body.trim().is_empty() {
                .body.markdownRender
                    data-markdown-source=(compact_body.as_str())
                    data-show=(compact_mode_expr.as_str())
                    style="display: none"
                {}
            }
            @if !full_body.trim().is_empty() {
                .body.is-full.markdownRender
                    data-markdown-source=(full_body)
                    data-show=(full_mode_expr.as_str())
                    style="display: none"
                {}
            }
            @if let Some(comments_count) = row.comments_count.filter(|value| *value > 0) {
                .small { (comments_count) " comments" }
            }
        }
    }
}

fn render_empty_state(is_empty: bool) -> Markup {
    html! {
        @if is_empty {
            .panel {
                p.small { "No tasks matched the current filters." }
            }
        }
    }
}

fn render_error_state(message: &str) -> Markup {
    html! {
        .warn {
            .header {
                .headerMeta {
                    h2.warnTitle { "Stream unavailable" }
                    p.small { (message) }
                }
            }
        }
    }
}

fn render_mismatch_state(expected_columns: &[&'static str], received_columns: &[String]) -> Markup {
    html! {
        .warn {
            .header {
                .headerMeta {
                    h2.warnTitle { "Snapshot mismatch" }
                    p.small { "The Kanban stream did not satisfy the Record-centric contract." }
                }
            }
            p.small { "Expected columns: " (expected_columns.join(", ")) }
            p.small { "Received columns: " (received_columns.join(", ")) }
        }
    }
}

fn rows_for_lane<'a>(lane: Lane, rows: &'a [KanbanRow]) -> Vec<&'a KanbanRow> {
    let mut grouped = rows
        .iter()
        .filter(|row| quantity_to_lane_key(row.quantity) == lane.key)
        .collect::<Vec<_>>();
    grouped.sort_by(|left, right| {
        left.head
            .to_lowercase()
            .cmp(&right.head.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    grouped
}

fn quantity_to_lane_key(quantity: i64) -> &'static str {
    if quantity > 0 {
        return "done";
    }
    match quantity {
        0 => "backlog",
        -1 => "next",
        -2 => "wip",
        -3 => "review",
        value if value < -3 => "review",
        _ => "backlog",
    }
}

fn compact_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut excerpt = Vec::new();
    let mut length = 0usize;
    for line in trimmed.lines().map(str::trim_end) {
        excerpt.push(line);
        length += line.len();
        if excerpt.len() >= 4 || length >= 220 {
            break;
        }
    }

    let compact = excerpt.join("\n").trim().to_string();
    if compact.len() < trimmed.len() {
        format!("{compact}\n...")
    } else {
        compact
    }
}

fn parse_i64(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("null") {
        return None;
    }
    raw.parse::<i64>().ok()
}

fn normalize_optional(raw: Option<&String>) -> Option<String> {
    let value = normalize_text(raw);
    if value.is_empty() { None } else { Some(value) }
}

fn normalize_text(raw: Option<&String>) -> String {
    let Some(value) = raw else {
        return String::new();
    };
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("null") {
        String::new()
    } else {
        value.clone()
    }
}

fn parse_json_strings(raw: Option<&String>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
        return Vec::new();
    }

    serde_json::from_str::<Vec<String>>(trimmed)
        .map(|values| {
            let mut seen = BTreeSet::new();
            values
                .into_iter()
                .filter_map(|value| {
                    let trimmed = value.trim().to_string();
                    if trimmed.is_empty() {
                        return None;
                    }
                    let lowered = trimmed.to_lowercase();
                    if seen.insert(lowered) {
                        Some(value)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn categories_for_row(row: &KanbanRow) -> Vec<String> {
    let mut categories = Vec::new();
    if let Some(primary) = row
        .primary_category
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        categories.push(primary.clone());
    }
    for category in &row.categories {
        if categories
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(category))
        {
            continue;
        }
        categories.push(category.clone());
    }
    categories
}

fn query_looks_filtered(query: &str) -> bool {
    let lowered = query.to_lowercase();
    lowered.contains("where 1 = 1")
        || lowered.contains("text_query")
        || lowered.contains("categories_any_json")
        || lowered.contains("assignee_ids_any_json")
}

#[cfg(test)]
mod tests {
    use super::render_sync_payload;

    #[test]
    fn renders_parent_label_when_parent_metadata_is_present() {
        let payload = r#"{
            "view_id": 7,
            "name": "Board",
            "query": "SELECT * FROM record WHERE 1 = 1",
            "columns": ["id", "quantity", "head", "body"],
            "rows": [
                {
                    "id": "1",
                    "quantity": "0",
                    "head": "Child",
                    "body": "",
                    "parent_id": "9",
                    "parent_head": "Parent task"
                }
            ]
        }"#;

        let rendered = render_sync_payload(payload, true).expect("payload should render");
        assert!(rendered.html.columns.contains("Parent task"));
        assert!(rendered.html.columns.contains("cardParentLink"));
    }

    #[test]
    fn renders_parent_label_even_without_parent_id() {
        let payload = r#"{
            "view_id": 7,
            "name": "Board",
            "query": "SELECT * FROM record WHERE 1 = 1",
            "columns": ["id", "quantity", "head", "body"],
            "rows": [
                {
                    "id": "1",
                    "quantity": "0",
                    "head": "Child",
                    "body": "",
                    "parent_head": "Parent task"
                }
            ]
        }"#;

        let rendered = render_sync_payload(payload, true).expect("payload should render");
        assert!(rendered.html.columns.contains("Parent task"));
        assert!(rendered.html.columns.contains("<span class=\"cardParentLink\">"));
    }
}
