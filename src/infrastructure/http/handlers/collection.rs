use axum::{extract::Path, response::Html};

use crate::{
    application::use_cases::collection::set_active::use_case_collection_set_active,
    presentation::web::collection::{
        presentation_web_collection_hovered, presentation_web_collection_unhovered,
    },
};

pub async fn handler_collection_unhovered() -> Html<String> {
    Html(presentation_web_collection_unhovered().await.0)
}

pub async fn handler_collection_hovered() -> Html<String> {
    Html(presentation_web_collection_hovered().await.0)
}

pub async fn handler_collection_set_active(Path(id): Path<String>) -> Html<String> {
    Html(use_case_collection_set_active(id).await.to_string())
}
