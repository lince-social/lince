use crate::application::{
    providers::configuration::set_active::provider_configuration_set_active,
    use_cases::section::body::use_case_section_get_body,
};

pub async fn use_case_configuration_set_active(id: String) -> &'static str {
    provider_configuration_set_active(id).await;
    use_case_section_get_body()
}
