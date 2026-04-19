use {
    crate::{
        application::kanban_filters::{
            KanbanWidgetSettings, derived_kanban_view_name, effective_kanban_view_id,
            extract_kanban_settings,
        },
        application::kanban_identity::is_supported_graph_widget_filename,
        domain::board::{BoardCard, BoardState},
        infrastructure::{
            auth::{AppAuth, RemoteServerSessionSnapshot, RemoteServerSessionState},
            board_state_store::BoardStateStore,
            organ_store::{OrganStore, organ_requires_auth},
        },
    },
    serde::Serialize,
    serde_json::Value,
};

const HEARTBEAT_INTERVAL_SECONDS: u64 = 15;
const STALE_AFTER_SECONDS: u64 = 45;
const DISCONNECTED_AFTER_SECONDS: u64 = 90;

#[derive(Clone)]
pub struct WidgetRuntimeService {
    auth: AppAuth,
    board_state: BoardStateStore,
    local_auth_required: bool,
    organs: OrganStore,
}

impl WidgetRuntimeService {
    pub fn new(
        auth: AppAuth,
        board_state: BoardStateStore,
        local_auth_required: bool,
        organs: OrganStore,
    ) -> Self {
        Self {
            auth,
            board_state,
            local_auth_required,
            organs,
        }
    }

    pub async fn kanban_contract(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
    ) -> Result<KanbanWidgetContract, WidgetRuntimeError> {
        let instance_id = instance_id.trim();
        if instance_id.is_empty() {
            return Err(WidgetRuntimeError::NotFound(
                "Widget instance ausente.".into(),
            ));
        }

        let board_state = self.board_state.snapshot().await;
        let card = find_board_card(&board_state, instance_id).ok_or_else(|| {
            WidgetRuntimeError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        validate_kanban_card(&card)?;

        let server_id = card.server_id.trim().to_string();
        if server_id.is_empty() {
            return Err(WidgetRuntimeError::Misconfigured(
                "Kanban sem server_id configurado no host.".into(),
            ));
        }

        let Some(view_id) = effective_kanban_view_id(&card.widget_state, card.view_id) else {
            return Err(WidgetRuntimeError::Misconfigured(
                "Kanban sem view_id valido configurado no host.".into(),
            ));
        };

        let organ = self
            .organs
            .get(&server_id)
            .await
            .map_err(WidgetRuntimeError::Internal)?
            .ok_or_else(|| {
                WidgetRuntimeError::Misconfigured(
                    "O server_id configurado no Kanban nao existe mais.".into(),
                )
            })?;

        let requires_auth = organ_requires_auth(&organ, self.local_auth_required);
        let session = self
            .auth
            .remote_server_snapshots(session_token)
            .await
            .get(&server_id)
            .cloned();
        let authenticated = !requires_auth || session.as_ref().is_some_and(is_connected);
        let filters = extract_filter_rows(&card.widget_state);
        let filters_version = extract_filters_version(&card.widget_state);
        let extracted_settings = extract_kanban_settings(&card.widget_state);
        let settings = KanbanWidgetSettings {
            view_name: Some(derived_kanban_view_name(
                &card.id,
                extracted_settings.view_name.as_deref(),
            )),
            ..extracted_settings
        };

        Ok(KanbanWidgetContract {
            widget: KanbanWidgetMeta {
                instance_id: card.id.clone(),
                title: card.title.clone(),
                description: card.description.clone(),
                package_name: card.package_name.clone(),
                sand_class: "engineer_with_clown_traits",
            },
            source: KanbanWidgetSource {
                server_id,
                server_name: organ.name,
                view_id,
                card_streams_enabled: card.streams_enabled,
                global_streams_enabled: board_state.global_streams_enabled,
                effective_streams_enabled: board_state.global_streams_enabled
                    && card.streams_enabled,
                requires_auth,
                authenticated,
                session_state: session.as_ref().map(session_state_name).map(str::to_string),
                username_hint: session
                    .as_ref()
                    .map(|value| value.username_hint.clone())
                    .unwrap_or_default(),
                connected_at_unix: session.as_ref().and_then(|value| value.connected_at_unix),
                last_error: session.map(|value| value.last_error).unwrap_or_default(),
            },
            permissions: KanbanWidgetPermissions {
                declared: card.permissions.clone(),
                read_view_stream: has_permission(&card, "read_view_stream"),
                write_records: has_permission(&card, "write_records"),
                write_table: has_permission(&card, "write_table"),
            },
            settings,
            data_contract: KanbanDataContract {
                required_columns: vec!["id", "quantity", "head", "body"],
                optional_columns: vec![
                    "primary_category",
                    "categories_json",
                    "start_at",
                    "end_at",
                    "estimate_seconds",
                    "actual_seconds",
                    "assignee_ids_json",
                    "assignee_names_json",
                    "parent_id",
                    "parent_head",
                    "parent_ids_json",
                    "parent_heads_json",
                    "depth",
                    "children_count",
                    "children_json",
                    "task_type",
                    "comments_count",
                    "last_comment_preview",
                    "active_worklog_count",
                ],
                task_type_enum: vec!["epic", "feature", "task", "other"],
                quantity_mapping: vec![
                    KanbanQuantityColumn {
                        quantity: 0,
                        label: "Backlog",
                    },
                    KanbanQuantityColumn {
                        quantity: -1,
                        label: "Next",
                    },
                    KanbanQuantityColumn {
                        quantity: -2,
                        label: "WIP",
                    },
                    KanbanQuantityColumn {
                        quantity: -3,
                        label: "Review",
                    },
                    KanbanQuantityColumn {
                        quantity: 1,
                        label: "Done",
                    },
                ],
            },
            filters: KanbanFilterContract {
                fields: vec![
                    "text_query",
                    "categories_any_json",
                    "assignee_ids_any_json",
                    "quantities_json",
                    "task_types_json",
                    "only_with_open_worklog",
                ],
                row_logic: "and",
                multi_value_logic: "or",
                filters_version,
                rows: filters,
            },
            relations: vec![
                KanbanRelationContract {
                    name: "task_categories",
                    storage: "record_extension:task.categories",
                    cardinality: "many",
                },
                KanbanRelationContract {
                    name: "task_assignees",
                    storage: "record_link:assigned_to->app_user",
                    cardinality: "many",
                },
                KanbanRelationContract {
                    name: "task_parent",
                    storage: "record_link:parent->record",
                    cardinality: "one",
                },
                KanbanRelationContract {
                    name: "task_comments",
                    storage: "record_comment",
                    cardinality: "many",
                },
                KanbanRelationContract {
                    name: "task_resources",
                    storage: "record_resource_ref",
                    cardinality: "many",
                },
                KanbanRelationContract {
                    name: "task_worklog",
                    storage: "record_worklog",
                    cardinality: "many",
                },
            ],
            actions: vec![
                "load-record-detail",
                "load-form-options",
                "create-record",
                "update-record",
                "update-record-body",
                "move-record",
                "delete-record",
                "start-worklog",
                "stop-worklog",
                "heartbeat-worklog",
                "create-comment",
                "update-comment",
                "delete-comment",
                "create-resource-ref",
                "delete-resource-ref",
                "apply-filters",
                "update-settings",
            ],
            liveness: KanbanLivenessContract {
                heartbeat_interval_seconds: HEARTBEAT_INTERVAL_SECONDS,
                stale_after_seconds: STALE_AFTER_SECONDS,
                disconnected_after_seconds: DISCONNECTED_AFTER_SECONDS,
            },
            form_options: None,
            diagnostics: KanbanDiagnostics {
                effective_sql: None,
            },
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanWidgetContract {
    pub widget: KanbanWidgetMeta,
    pub source: KanbanWidgetSource,
    pub permissions: KanbanWidgetPermissions,
    pub settings: KanbanWidgetSettings,
    pub data_contract: KanbanDataContract,
    pub filters: KanbanFilterContract,
    pub relations: Vec<KanbanRelationContract>,
    pub actions: Vec<&'static str>,
    pub liveness: KanbanLivenessContract,
    pub form_options: Option<Value>,
    pub diagnostics: KanbanDiagnostics,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanWidgetMeta {
    pub instance_id: String,
    pub title: String,
    pub description: String,
    pub package_name: String,
    pub sand_class: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanWidgetSource {
    pub server_id: String,
    pub server_name: String,
    pub view_id: u32,
    pub card_streams_enabled: bool,
    pub global_streams_enabled: bool,
    pub effective_streams_enabled: bool,
    pub requires_auth: bool,
    pub authenticated: bool,
    pub session_state: Option<String>,
    pub username_hint: String,
    pub connected_at_unix: Option<u64>,
    pub last_error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanWidgetPermissions {
    pub declared: Vec<String>,
    pub read_view_stream: bool,
    pub write_records: bool,
    pub write_table: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanDataContract {
    pub required_columns: Vec<&'static str>,
    pub optional_columns: Vec<&'static str>,
    pub task_type_enum: Vec<&'static str>,
    pub quantity_mapping: Vec<KanbanQuantityColumn>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanQuantityColumn {
    pub quantity: i32,
    pub label: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanFilterContract {
    pub fields: Vec<&'static str>,
    pub row_logic: &'static str,
    pub multi_value_logic: &'static str,
    pub filters_version: u64,
    pub rows: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanRelationContract {
    pub name: &'static str,
    pub storage: &'static str,
    pub cardinality: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanLivenessContract {
    pub heartbeat_interval_seconds: u64,
    pub stale_after_seconds: u64,
    pub disconnected_after_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KanbanDiagnostics {
    pub effective_sql: Option<String>,
}

#[derive(Debug, Clone)]
pub enum WidgetRuntimeError {
    NotFound(String),
    Misconfigured(String),
    Unsupported(String),
    Internal(String),
}

fn find_board_card(board_state: &BoardState, instance_id: &str) -> Option<BoardCard> {
    board_state
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.cards.iter())
        .find(|card| card.id == instance_id)
        .cloned()
}

fn validate_kanban_card(card: &BoardCard) -> Result<(), WidgetRuntimeError> {
    if card.kind.trim() != "package" {
        return Err(WidgetRuntimeError::Unsupported(
            "Esse widget nao e um package oficial.".into(),
        ));
    }

    if !is_supported_graph_widget_filename(&card.package_name) {
        return Err(WidgetRuntimeError::Unsupported(
            "Esse widget nao usa um package oficial suportado.".into(),
        ));
    }

    Ok(())
}

fn is_connected(session: &RemoteServerSessionSnapshot) -> bool {
    matches!(session.session_state, RemoteServerSessionState::Connected)
}

fn session_state_name(session: &RemoteServerSessionSnapshot) -> &'static str {
    match session.session_state {
        RemoteServerSessionState::Connected => "connected",
        RemoteServerSessionState::LoggedOut => "logged_out",
        RemoteServerSessionState::Expired => "expired",
    }
}

fn has_permission(card: &BoardCard, permission: &str) -> bool {
    card.permissions.iter().any(|value| value == permission)
}

fn extract_filters_version(widget_state: &Value) -> u64 {
    widget_state
        .get("filters_version")
        .and_then(Value::as_u64)
        .or_else(|| widget_state.get("filtersVersion").and_then(Value::as_u64))
        .unwrap_or(0)
}

fn extract_filter_rows(widget_state: &Value) -> Vec<Value> {
    widget_state
        .get("filters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}
