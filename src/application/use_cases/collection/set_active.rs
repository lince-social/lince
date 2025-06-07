use crate::{
    application::providers::collection::set_active::provider_collection_set_active,
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::section::body::presentation_web_section_body,
};

pub async fn use_case_collection_set_active(services: InjectedServices, id: String) -> String {
    provider_collection_set_active(id).await;
    presentation_web_section_body(services).await
}
