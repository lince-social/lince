use crate::{
    application::providers::configuration::set_active::provider_configuration_set_active,
    presentation::web::section::body::presentation_web_section_body,
};

pub async fn use_case_configuration_set_active(id: String) -> &'static str {
    provider_configuration_set_active(id).await;
    presentation_web_section_body()
}
