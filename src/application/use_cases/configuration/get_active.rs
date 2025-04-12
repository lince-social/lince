use crate::{
    application::providers::configuration::get_active::provider_configuration_get_active,
    presentation::web::configuration::configurations::presentation_web_configuration_unhovered,
};

pub async fn use_case_configuration_get_active() -> String {
    let (active_configuration, active_configuration_views) =
        provider_configuration_get_active().await;
    presentation_web_configuration_unhovered(active_configuration, active_configuration_views)
        .await
        .0
}
