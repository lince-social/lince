use crate::{
    application::providers::configuration::{
        get_active::provider_configuration_get_active,
        get_inactive::provider_configuration_get_inactive,
    },
    presentation::web::configuration::configurations::presentation_web_configuration_hovered,
};

pub async fn use_case_configuration_get_inactive() -> String {
    let active_configuration = provider_configuration_get_active().await;
    let inactive_configurations = provider_configuration_get_inactive().await;
    presentation_web_configuration_hovered(active_configuration, inactive_configurations)
        .await
        .0
}
