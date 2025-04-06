use crate::{
    application::providers::record::zero_quantity::provider_record_zero_quantity,
    presentation::web::section::body::presentation_web_section_body,
};

pub async fn use_case_record_zero_quantity(id: String) -> String {
    provider_record_zero_quantity(id).await;
    presentation_web_section_body().to_string()
}
