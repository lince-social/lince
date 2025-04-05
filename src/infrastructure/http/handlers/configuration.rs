use crate::presentation::web::record::record::get_records_component;

pub async fn get_configuration_handler() -> String {
    get_records_component().await.0
}
