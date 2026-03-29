use crate::{
    application::ai_builder::AiBuilderState,
    application::backend_api::BackendApiService,
    application::kanban_actions::KanbanActionService,
    application::kanban_filters::KanbanFilterService,
    application::kanban_streams::KanbanStreamService,
    application::widget_runtime::WidgetRuntimeService,
    infrastructure::{
        auth::AppAuth, board_state_store::BoardStateStore, manas::ManasGateway,
        organ_store::OrganStore, package_catalog_store::PackageCatalogStore,
        terminal_store::TerminalSessionStore, widget_bridge_store::WidgetBridgeStore,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub ai: AiBuilderState,
    pub auth: AppAuth,
    pub backend: BackendApiService,
    pub board_state: BoardStateStore,
    pub local_auth_required: bool,
    pub manas: ManasGateway,
    pub organs: OrganStore,
    pub packages: PackageCatalogStore,
    pub terminal: TerminalSessionStore,
    pub widget_bridge: WidgetBridgeStore,
    pub kanban_actions: KanbanActionService,
    pub kanban_filters: KanbanFilterService,
    pub kanban_streams: KanbanStreamService,
    pub widget_runtime: WidgetRuntimeService,
}

impl axum::extract::FromRef<AppState> for AiBuilderState {
    fn from_ref(input: &AppState) -> Self {
        input.ai.clone()
    }
}
