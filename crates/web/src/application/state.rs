use crate::{
    application::ai_builder::AiBuilderState,
    application::backend_api::BackendApiService,
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
}

impl axum::extract::FromRef<AppState> for AiBuilderState {
    fn from_ref(input: &AppState) -> Self {
        input.ai.clone()
    }
}
