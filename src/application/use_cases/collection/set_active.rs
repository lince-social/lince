use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::section::body::presentation_web_section_body,
};

pub async fn use_case_collection_set_active(services: InjectedServices, id: String) -> String {
    services.providers.collection.set_active(id).await;
    presentation_web_section_body(services).await
}
