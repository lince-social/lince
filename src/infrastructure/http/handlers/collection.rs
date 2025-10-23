use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::html::section::body::presentation_html_section_body,
};
use axum::{
    extract::{Path, State},
    response::Html,
};

pub async fn handler_collection_set_active(
    State(services): State<InjectedServices>,
    Path(id): Path<String>,
) -> Html<String> {
    let _ = services.repository.collection.set_active(&id).await;
    Html(presentation_html_section_body(services).await)
}
