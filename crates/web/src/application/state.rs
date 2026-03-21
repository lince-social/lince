use crate::{
    application::ai_builder::AiBuilderState,
    infrastructure::{
        auth::AppAuth, board_state_store::BoardStateStore, manas::ManasGateway,
        package_catalog_store::PackageCatalogStore,
        terminal_store::TerminalSessionStore,
        widget_bridge_store::WidgetBridgeStore,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub ai: AiBuilderState,
    pub auth: AppAuth,
    pub board_state: BoardStateStore,
    pub manas: ManasGateway,
    pub packages: PackageCatalogStore,
    pub terminal: TerminalSessionStore,
    pub widget_bridge: WidgetBridgeStore,
}

impl axum::extract::FromRef<AppState> for AiBuilderState {
    fn from_ref(input: &AppState) -> Self {
        input.ai.clone()
    }
}
