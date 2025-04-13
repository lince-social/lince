use crate::{
    application::providers::record::set_quantity::provider_record_set_quantity,
    presentation::web::section::body::presentation_web_section_body,
};

pub async fn use_case_record_set_quantity(id: String, quantity: f64) -> String {
    provider_record_set_quantity(id, quantity).await;
    presentation_web_section_body().to_string()
}
