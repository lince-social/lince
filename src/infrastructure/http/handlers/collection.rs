use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::section::body::presentation_web_section_body,
};
use axum::{
    extract::{Path, State},
    response::Html,
};

pub async fn handler_collection_set_active(
    State(services): State<InjectedServices>,
    Path(id): Path<String>,
) -> Html<String> {
    services.providers.collection.set_active(&id).await;
    Html(presentation_web_section_body(services).await)
}
