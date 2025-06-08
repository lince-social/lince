use crate::{
    infrastructure::cross_cutting::InjectedServices,
    presentation::web::section::body::presentation_web_section_body,
};

pub async fn use_case_record_set_quantity(
    services: InjectedServices,
    id: u32,
    quantity: f64,
) -> String {
    if let Err(e) = services
        .providers
        .record
        .set_quantity
        .execute(id, quantity)
        .await
    {
        println!(
            "Error when setting record with id: {} to quantity: {} | Error: {}",
            id, quantity, e
        )
    }
    presentation_web_section_body(services).await
}
