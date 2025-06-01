use crate::{
    application::providers::selection::set_active::provider_selection_set_active,
    presentation::web::section::body::presentation_web_section_body,
};

pub async fn use_case_selection_set_active(id: String) -> String {
    provider_selection_set_active(id).await;
    presentation_web_section_body().await
}
