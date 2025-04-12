use axum::{extract::Path, response::Html};

use crate::application::use_cases::configuration::{
    get_active::use_case_configuration_get_active,
    get_inactive::use_case_configuration_get_inactive,
    set_active::use_case_configuration_set_active,
};

pub async fn get_active_configuration_handler() -> Html<String> {
    Html(use_case_configuration_get_active().await)
}

pub async fn get_inactive_configurations_handler() -> Html<String> {
    Html(use_case_configuration_get_inactive().await)
}

pub async fn handler_configuration_set_active(Path(id): Path<String>) -> Html<String> {
    Html(use_case_configuration_set_active(id).await.to_string())
}
