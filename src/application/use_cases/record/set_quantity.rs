use crate::{
    application::providers::record::set_quantity::provider_record_set_quantity,
    presentation::web::section::body::presentation_web_section_body,
};

pub async fn use_case_record_set_quantity(id: u32, quantity: f64) -> String {
    if let Err(e) = provider_record_set_quantity(id, quantity).await {
        println!(
            "Error when setting record with id: {} to quantity: {} | Error: {}",
            id, quantity, e
        )
    }
    presentation_web_section_body().await
}
