use axum::response::Html;

use crate::application::use_cases::configuration::{
    get_active::use_case_configuration_get_active,
    get_inactive::use_case_configuration_get_inactive,
};

pub async fn get_active_configuration_handler() -> Html<String> {
    Html(use_case_configuration_get_active().await)
}

pub async fn get_inactive_configurations_handler() -> Html<String> {
    Html(use_case_configuration_get_inactive().await)
}
