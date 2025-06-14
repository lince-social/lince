use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::section::body::presentation_web_section_body,
};
use axum::extract::{Path, State};

pub async fn handler_view_toggle_view_id(
    State(services): State<InjectedServices>,
    Path((collection_id, view_id)): Path<(u32, u32)>,
) -> String {
    services
        .providers
        .view
        .toggle_by_view_id(collection_id, view_id)
        .await;

    presentation_web_section_body(services).await
}

pub async fn handler_view_toggle_collection_id(
    State(services): State<InjectedServices>,
    Path(id): Path<u32>,
) -> String {
    services.providers.view.toggle_by_collection_id(id).await;
    presentation_web_section_body(services).await
}
