use axum::{extract::Path, response::Html};

use crate::{
    application::use_cases::selection::set_active::use_case_selection_set_active,
    presentation::web::selection::{
        presentation_web_selection_hovered, presentation_web_selection_unhovered,
    },
};

pub async fn handler_selection_unhovered() -> Html<String> {
    Html(presentation_web_selection_unhovered().await.0)
}

pub async fn handler_selection_hovered() -> Html<String> {
    Html(presentation_web_selection_hovered().await.0)
}

pub async fn handler_selection_set_active(Path(id): Path<String>) -> Html<String> {
    Html(use_case_selection_set_active(id).await.to_string())
}
