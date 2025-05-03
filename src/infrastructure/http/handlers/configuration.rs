use axum::{extract::Path, response::Html};

use crate::{
    application::use_cases::configuration::set_active::use_case_configuration_set_active,
    presentation::web::configuration::configurations::{
        presentation_web_configuration_hovered, presentation_web_configuration_unhovered,
    },
};

pub async fn handler_configuration_unhovered() -> Html<String> {
    Html(presentation_web_configuration_unhovered().await.0)
}

pub async fn handler_configuration_hovered() -> Html<String> {
    Html(presentation_web_configuration_hovered().await.0)
}

pub async fn handler_configuration_set_active(Path(id): Path<String>) -> Html<String> {
    Html(use_case_configuration_set_active(id).await.to_string())
}
